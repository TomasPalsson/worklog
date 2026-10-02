"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { ArrowUpRight, ChevronRight } from "lucide-react";

import { formatDuration, formatRange, shortMonthDay, shortWeekday } from "@/lib/format";
import { chipOf, type Chip } from "@/lib/ticketHours";
import type { JiraTicket, RawBlock, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { BlockMove } from "./TaskBlockMove";
import { DayMenu } from "./TaskDayMenu";
import { SyncBody, SyncTrigger, useSync } from "./TaskDaySync";
import { DaySent, HoursEdit, TextEdit, hoursNote, useDayOps, useHoursWrite } from "./TaskDayTools";
import { changedNote } from "./TaskSyncPreview";
import { useOverflow } from "./useOverflow";

export { chipOf, type Chip };

export const CHIP_TONE: Record<Chip, string> = { Sent: "ok", "Changed since sent": "changed", "Not sent": "none" };

/** "Thu 1 Oct". */
export const dayLabel = (day: string) =>
  `${shortWeekday(day)} ${Number(day.slice(8))} ${shortMonthDay(day).split(" ")[0]}`;

/** Clamped to two lines; "more" appears only when the text really overflows (measured, not guessed). */
function LineText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const span = useRef<HTMLSpanElement>(null);
  const overflows = useOverflow(span, text, open);
  return (
    <p className="task-day-text" data-open={open || undefined}>
      <span ref={span}>{text}</span>
      {overflows && (
        <button type="button" className="task-link-btn" onClick={() => setOpen((o) => !o)}>
          {open ? "less" : "more"}
        </button>
      )}
    </p>
  );
}

interface BlockRowProps {
  block: RawBlock;
  fresh: boolean;
  tickets: JiraTicket[];
  onMoved: () => void | Promise<void>;
  onAnnounce?: (message: string) => void;
}

function BlockRow({ block, fresh, tickets, onMoved, onAnnounce }: BlockRowProps) {
  const row = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    if (fresh) row.current?.scrollIntoView?.({ block: "nearest" });
  }, [fresh]);
  return (
    <li className="task-block-item">
      <Link ref={row} href={`/${block.day}/block/${block.id}`} className="task-block-row" data-fresh={fresh || undefined}>
        <span className="task-block-range">{formatRange(block.started_at, block.ended_at)}</span>
        <span className="task-block-dur">{formatDuration(block.duration_seconds)}</span>
        {block.description ? (
          <span className="task-block-desc" title={block.description}>
            {block.description}
          </span>
        ) : (
          <span className="task-block-desc task-block-none">No description</span>
        )}
        {block.estimated_by === "manual" && <span className="task-block-tag">Edited</span>}
        <span className="task-block-go" title="Opens the block page">
          <ArrowUpRight size={14} aria-hidden="true" />
          <span className="task-sr">Opens the block page</span>
        </span>
      </Link>
      <BlockMove block={block} tickets={tickets} onMoved={onMoved} onAnnounce={onAnnounce} />
    </li>
  );
}

interface DayGroupProps {
  day: TicketDay;
  taskKey: string;
  actions: TaskActions;
  onSaved: () => void;
  onAnnounce?: (message: string) => void;
  /** The block just logged through the form, if any. */
  logged?: { id: number; day: string; duration: string } | null;
  expanded: boolean;
  onExpand: (open: boolean) => void;
  /** The board's tickets, offered first in the Move picker; live Jira search covers the rest. */
  tickets?: JiraTicket[];
}

/** One day: a single disclosure row (hours, Tempo state, next action, "⋯"), then what the row opened, then the details. */
export function DayGroup({ day, taskKey, actions, onSaved, onAnnounce, logged, expanded, onExpand, tickets = [] }: DayGroupProps) {
  const chip = chipOf(day.blocks);
  const label = dayLabel(day.day);
  const changed = chip === "Changed since sent";
  const tools = { taskKey, actions, onSaved, onAnnounce, label, day };
  const sync = useSync({ ...tools, inTempo: chip === "Sent", changed });
  const hours = useHoursWrite(tools);
  const ops = useDayOps(tools);
  const [editing, setEditing] = useState<"hours" | "text" | null>(null);
  const more = useRef<HTMLButtonElement>(null);
  const done = () => {
    setEditing(null);
    more.current?.focus();
  };
  const edit = (what: "hours" | "text") => {
    hours.setError(null);
    setEditing(what);
  };
  // The preview points at the day's text below it, so opening one opens the day.
  const trigger = { ...sync, dryRun: () => (onExpand(true), sync.dryRun()) };
  return (
    <div className="task-day" data-state={CHIP_TONE[chip]} data-day={day.day}>
      <div className="task-day-row">
        <h4 className="task-day-head">
          <button type="button" className="task-day-toggle" aria-expanded={expanded} onClick={() => onExpand(!expanded)}>
            <ChevronRight size={14} className="task-day-chev" aria-hidden="true" />
            <span className="task-day-label">{label}</span>
            <span className="task-day-hours">{formatDuration(day.line_seconds)}</span>
            <span className="task-day-chip" data-chip={CHIP_TONE[chip]} title={changed ? changedNote(day) : undefined}>
              <span className="task-chip-long">{chip}</span>
              {changed && (
                <span className="task-chip-short" aria-hidden="true">
                  Changed
                </span>
              )}
            </span>
          </button>
        </h4>
        <SyncTrigger sync={trigger} label={label} inTempo={chip === "Sent"} changed={changed} />
        <DayMenu
          label={label}
          byHand={day.hours_set_by_hand}
          busy={hours.busy || !!ops.busy}
          ops={ops}
          btn={more}
          onEditHours={() => edit("hours")}
          onUseTracked={() => hours.write(null)}
          onEditText={() => edit("text")}
        />
      </div>
      {ops.busy && <DaySent plain>{ops.busy}</DaySent>}
      {logged?.day === day.day && <DaySent>{`Logged ${logged.duration}`}</DaySent>}
      <SyncBody sync={sync} label={label} day={day} changed={changed} />
      {editing === "hours" && <HoursEdit label={label} day={day} io={hours} onDone={done} />}
      {editing === "text" && <TextEdit {...tools} onDone={done} />}
      {editing !== "hours" && hours.error && (
        <p role="alert" className="task-error">
          {hours.error}
        </p>
      )}
      {expanded && (
        <DayBody day={day} editingText={editing === "text"} freshId={logged?.id} tickets={tickets} onMoved={onSaved} onAnnounce={onAnnounce} />
      )}
    </div>
  );
}

/** What an open day adds: why the hours are what they are, the Tempo text, and the blocks. */
function DayBody(p: Pick<BlockRowProps, "tickets" | "onMoved" | "onAnnounce"> & { day: TicketDay; editingText: boolean; freshId?: number }) {
  const { day, editingText, freshId, tickets, onMoved, onAnnounce } = p;
  const note = hoursNote(day);
  return (
    <div className="task-day-body">
      {!editingText && day.line_text && <LineText text={day.line_text} />}
      {note && <p className="task-day-note">{note}</p>}
      {chipOf(day.blocks) === "Changed since sent" && <p className="task-day-note">{changedNote(day)}</p>}
      <ul className="task-block-list">
        {day.blocks.map((b) => (
          <BlockRow key={b.id} block={b} fresh={freshId === b.id} tickets={tickets} onMoved={onMoved} onAnnounce={onAnnounce} />
        ))}
      </ul>
    </div>
  );
}
