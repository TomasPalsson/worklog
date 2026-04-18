"use client";

import { useRef, useState, useTransition } from "react";
import { Check, Clock, Trash2 } from "lucide-react";
import type { Block, JiraTicket } from "@/lib/types";
import { formatDuration, formatRange } from "@/lib/format";
import { deleteBlock, setDescription, setDuration } from "@/app/actions";
import { SourceBadges } from "./SourceBadges";
import { EstBadge } from "./EstBadge";
import { TicketCombobox } from "./TicketCombobox";

interface Props {
  block: Block;
  tickets: JiraTicket[];
  day: string;
}

export function BlockCard({ block, tickets, day }: Props) {
  const [editingDur, setEditingDur] = useState(false);
  const [durVal, setDurVal] = useState(Math.round(block.duration_seconds / 60));
  const [, start] = useTransition();
  const descRef = useRef<HTMLDivElement>(null);

  const assigned = !!block.jira_issue;
  const synced = !!block.tempo_worklog_id;
  const cls = ["block", assigned ? "assigned" : "unassigned", synced ? "synced" : ""]
    .filter(Boolean)
    .join(" ");

  const commitDescription = () => {
    const next = (descRef.current?.innerText ?? "").trim();
    if (next === (block.description ?? "")) return;
    start(() => setDescription(block.id, next, day));
  };

  const commitDuration = () => {
    setEditingDur(false);
    const m = Math.max(1, durVal | 0);
    if (m === Math.round(block.duration_seconds / 60)) return;
    start(() => setDuration(block.id, m, day));
  };

  return (
    <article className={cls} aria-label={`block ${block.id}`}>
      <div className="block-time">
        <span className="range">{formatRange(block.started_at, block.ended_at)}</span>
        {editingDur ? (
          <span className="duration-edit">
            <input
              type="number"
              min={1}
              value={durVal}
              autoFocus
              onChange={(e) => setDurVal(Number(e.target.value))}
              onBlur={commitDuration}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitDuration();
                if (e.key === "Escape") {
                  setDurVal(Math.round(block.duration_seconds / 60));
                  setEditingDur(false);
                }
              }}
            />
            <span className="unit">min</span>
          </span>
        ) : (
          <button
            type="button"
            className="duration"
            title="Click to edit"
            onClick={() => setEditingDur(true)}
          >
            {formatDuration(block.duration_seconds)}
          </button>
        )}
      </div>

      <div className="block-body">
        <div className="block-title-row">
          <TicketCombobox
            blockId={block.id}
            current={block.jira_issue}
            tickets={tickets}
            day={day}
          />
          {synced && (
            <span className="synced-tag" title={`Synced to Tempo · id ${block.tempo_worklog_id}`}>
              <Check />
              synced
            </span>
          )}
        </div>

        <div
          ref={descRef}
          className={`block-description ${!block.description ? "empty" : ""}`}
          contentEditable
          suppressContentEditableWarning
          spellCheck={false}
          onBlur={commitDescription}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              (e.target as HTMLDivElement).blur();
            }
          }}
        >
          {block.description ?? "Click to add a description…"}
        </div>

        <div className="block-meta">
          <SourceBadges sources={block.sources} />
          <EstBadge kind={block.estimated_by} />
          {block.event_count > 0 && (
            <span title="total events in this block" style={{ color: "var(--fg-subtle)" }}>
              <Clock
                width={11}
                height={11}
                style={{ display: "inline", verticalAlign: "-1px", marginRight: 3 }}
              />
              {block.event_count} event{block.event_count === 1 ? "" : "s"}
            </span>
          )}
        </div>
      </div>

      <div className="block-actions">
        <button
          type="button"
          className="icon-btn danger"
          title="Delete block"
          aria-label="delete block"
          onClick={() => {
            if (confirm(`Delete block ${block.id}? This also removes links to its events.`)) {
              start(() => deleteBlock(block.id, day));
            }
          }}
        >
          <Trash2 />
        </button>
      </div>
    </article>
  );
}
