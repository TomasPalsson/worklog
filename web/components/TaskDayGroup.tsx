"use client";

import { useState } from "react";
import Link from "next/link";

import { formatDuration, formatRange, shortMonthDay, shortWeekday } from "@/lib/format";
import type { RawBlock, TicketDay } from "@/lib/types";
import type { TaskActions } from "./TaskCard";
import { HoursEdit, SyncTool, TextEdit } from "./TaskDayTools";

type Chip = "In Tempo" | "Changed since sync" | "Not synced";

const synced = (b: RawBlock) => !!b.tempo_worklog_id;

function chipOf(blocks: RawBlock[]): Chip {
  if (blocks.some((b) => synced(b) && b.dirty)) return "Changed since sync";
  return blocks.every(synced) ? "In Tempo" : "Not synced";
}

const CHIP_TONE: Record<Chip, string> = { "In Tempo": "ok", "Changed since sync": "changed", "Not synced": "none" };

/** "Thu 1 Oct". */
const dayLabel = (day: string) =>
  `${shortWeekday(day)} ${Number(day.slice(8))} ${shortMonthDay(day).split(" ")[0]}`;

const CLAMP_CHARS = 140;

function LineText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <p className="task-day-text" data-open={open || undefined}>
      <span>{text}</span>
      {text.length > CLAMP_CHARS && (
        <button type="button" className="task-link-btn" onClick={() => setOpen((o) => !o)}>
          {open ? "less" : "more"}
        </button>
      )}
    </p>
  );
}

function BlockRow({ block }: { block: RawBlock }) {
  return (
    <li>
      <Link href={`/${block.day}/block/${block.id}`} className="task-block-row">
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
      </Link>
    </li>
  );
}

export function DayGroup({
  day,
  taskKey,
  actions,
  onSaved,
}: {
  day: TicketDay;
  taskKey: string;
  actions: TaskActions;
  onSaved: () => void;
}) {
  const chip = chipOf(day.blocks);
  const label = dayLabel(day.day);
  const tools = { taskKey, actions, onSaved, label, day };
  return (
    <div className="task-day">
      <div className="task-day-head">
        <Link href={`/${day.day}`} className="task-day-label">
          {label}
        </Link>
        <HoursEdit {...tools} />
        <span className="task-day-chip" data-chip={CHIP_TONE[chip]}>
          {chip}
        </span>
        <TextEdit {...tools} />
      </div>
      {day.line_text && <LineText text={day.line_text} />}
      <SyncTool {...tools} inTempo={chip === "In Tempo"} />
      <ul className="task-block-list">
        {day.blocks.map((b) => (
          <BlockRow key={b.id} block={b} />
        ))}
      </ul>
    </div>
  );
}
