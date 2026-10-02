"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { ChevronRight } from "lucide-react";

import { formatDuration, formatRange, shortMonthDay, shortWeekday } from "@/lib/format";
import type { RawBlock, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { DaySent, HoursEdit, SyncTool, TextEdit, hoursNote } from "./TaskDayTools";
import { changedNote } from "./TaskSyncPreview";

type Chip = "In Tempo" | "Changed since sync" | "Not synced";

const synced = (b: RawBlock) => !!b.tempo_worklog_id;

function chipOf(blocks: RawBlock[]): Chip {
  if (blocks.some((b) => synced(b) && b.dirty)) return "Changed since sync";
  return blocks.every(synced) ? "In Tempo" : "Not synced";
}

const CHIP_TONE: Record<Chip, string> = { "In Tempo": "ok", "Changed since sync": "changed", "Not synced": "none" };

/** "Thu 1 Oct". */
export const dayLabel = (day: string) =>
  `${shortWeekday(day)} ${Number(day.slice(8))} ${shortMonthDay(day).split(" ")[0]}`;

/** Clamped to two lines; "more" appears only when the text really overflows (measured, not guessed). */
function LineText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const span = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = span.current;
    if (!el || open) return; // open text never overflows; keep the last measurement so "less" stays
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [text, open]);
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

function BlockRow({ block, fresh }: { block: RawBlock; fresh: boolean }) {
  const row = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    if (fresh) row.current?.scrollIntoView?.({ block: "nearest" });
  }, [fresh]);
  return (
    <li>
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
        <ChevronRight size={14} className="task-block-go" aria-hidden="true" />
      </Link>
    </li>
  );
}

export function DayGroup({
  day,
  taskKey,
  actions,
  onSaved,
  onAnnounce,
  logged,
}: {
  day: TicketDay;
  taskKey: string;
  actions: TaskActions;
  onSaved: () => void;
  onAnnounce?: (message: string) => void;
  /** The block just logged through the form, if any. */
  logged?: { id: number; day: string; duration: string } | null;
}) {
  const chip = chipOf(day.blocks);
  const label = dayLabel(day.day);
  const note = hoursNote(day);
  const changed = chip === "Changed since sync";
  const tools = { taskKey, actions, onSaved, onAnnounce, label, day };
  return (
    <div className="task-day">
      <div className="task-day-head">
        <h4 className="task-day-label">{label}</h4>
        <HoursEdit {...tools} />
        <span className="task-day-chip" data-chip={CHIP_TONE[chip]} title={changed ? changedNote(day) : undefined}>
          {chip}
        </span>
      </div>
      {note && <p className="task-day-note">{note}</p>}
      {changed && <p className="task-day-note">{changedNote(day)}</p>}
      {logged?.day === day.day && <DaySent>{`Logged ${logged.duration}`}</DaySent>}
      <TextEdit {...tools}>{day.line_text && <LineText text={day.line_text} />}</TextEdit>
      <SyncTool {...tools} inTempo={chip === "In Tempo"} changed={changed} />
      <ul className="task-block-list">
        {day.blocks.map((b) => (
          <BlockRow key={b.id} block={b} fresh={logged?.id === b.id} />
        ))}
      </ul>
    </div>
  );
}
