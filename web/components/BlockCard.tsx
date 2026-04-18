"use client";

import { useRef, useState, useTransition } from "react";
import { Check, Clock, Trash2 } from "lucide-react";
import type { Block, JiraTicket } from "@/lib/types";
import { formatDuration, formatRange } from "@/lib/format";
import { deleteBlock, setDescription, setDuration } from "@/app/actions";
import { toast } from "@/lib/toast";
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

  const timeRangeLabel = formatRange(block.started_at, block.ended_at);
  const durationLabel = formatDuration(block.duration_seconds);
  // Article label for screen readers — useful info, not "block 42".
  const ariaLabel = `${timeRangeLabel} · ${block.jira_issue ?? "unassigned"} · ${durationLabel}`;

  const commitDescription = () => {
    const previous = block.description ?? "";
    const next = (descRef.current?.innerText ?? "").trim();
    if (next === previous) return;
    start(async () => {
      const r = await setDescription(block.id, next, day);
      if (!r.ok) {
        toast.error(`Save description failed — ${r.error}`);
        // Revert the visible text so the user sees the real DB state,
        // not their unsaved edit. Without this the UI silently disagrees
        // with the DB and the next blur re-submits the same rejected text.
        if (descRef.current) {
          descRef.current.innerText = previous;
        }
      } else if (synced) {
        toast.ok(
          "Description saved. Note: this block was already synced — re-sync to update Tempo.",
        );
      }
    });
  };

  const commitDuration = () => {
    setEditingDur(false);
    const previousMinutes = Math.round(block.duration_seconds / 60);
    const m = Math.max(1, durVal | 0);
    if (m === previousMinutes) return;
    start(async () => {
      const r = await setDuration(block.id, m, day);
      if (!r.ok) {
        toast.error(`Save duration failed — ${r.error}`);
        // Revert local state so the next edit starts from the canonical
        // value, not the rejected one.
        setDurVal(previousMinutes);
      } else if (synced) {
        toast.ok(
          "Duration saved. Note: this block was already synced — re-sync to update Tempo.",
        );
      }
    });
  };

  const onDelete = () => {
    const label = `the ${timeRangeLabel} block${block.jira_issue ? ` on ${block.jira_issue}` : ""}`;
    if (!confirm(`Delete ${label}? This also removes links to its events.`)) return;
    start(async () => {
      const r = await deleteBlock(block.id, day);
      if (!r.ok) toast.error(`Delete failed — ${r.error}`);
    });
  };

  return (
    <article className={cls} aria-label={ariaLabel}>
      <div className="block-time">
        <span className="range" aria-label={`time range ${timeRangeLabel}`}>
          {timeRangeLabel}
        </span>
        {editingDur ? (
          <span className="duration-edit">
            <input
              type="number"
              min={1}
              value={durVal}
              autoFocus
              aria-label="duration in minutes"
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
            aria-label={`duration ${durationLabel} — click to edit`}
            onClick={() => setEditingDur(true)}
          >
            {durationLabel}
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
          role="textbox"
          aria-multiline="true"
          aria-label="Block description — click to edit"
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
          aria-label={`delete ${timeRangeLabel} block`}
          onClick={onDelete}
        >
          <Trash2 />
        </button>
      </div>
    </article>
  );
}
