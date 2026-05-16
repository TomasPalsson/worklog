"use client";

import { useRef, useState, useTransition } from "react";
import { Check, Coffee, Trash2 } from "lucide-react";
import type { Block, JiraTicket } from "@/lib/types";
import { formatDuration, formatRange } from "@/lib/format";
import {
  deleteBlock,
  setDescription,
  setDuration,
  setPersonal,
} from "@/app/actions";
import { toast } from "@/lib/toast";
import { SourceBadges } from "./SourceBadges";
import { EstBadge } from "./EstBadge";
import { TicketCombobox } from "./TicketCombobox";
import { EventList } from "./EventList";
import { CommitList } from "./CommitList";

interface Props {
  block: Block;
  tickets: JiraTicket[];
  day: string;
}

export function BlockCard({ block, tickets, day }: Props) {
  const [editingDur, setEditingDur] = useState(false);
  const [durVal, setDurVal] = useState(Math.round(block.duration_seconds / 60));
  const [isPending, start] = useTransition();
  const descRef = useRef<HTMLDivElement>(null);

  const assigned = !!block.jira_issue;
  const synced = !!block.tempo_worklog_id;
  const dirty = synced && block.dirty;
  const cls = [
    "block",
    assigned ? "assigned" : "unassigned",
    synced ? "synced" : "",
    dirty ? "dirty" : "",
    block.is_personal ? "personal" : "",
  ]
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

  const onTogglePersonal = () => {
    const next = !block.is_personal;
    start(async () => {
      const r = await setPersonal(block.id, next, day);
      if (!r.ok) {
        toast.error(
          `${next ? "Mark personal" : "Mark as work"} failed — ${r.error}`,
        );
      } else if (next && synced) {
        // A synced block keeps its Tempo entry — flipping it personal
        // here only stops *future* syncs, it doesn't retract the past one.
        toast.ok(
          "Marked personal. Note: this block is already synced — its Tempo entry stays; delete the block to remove it.",
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
          {synced && !dirty && (
            <span className="synced-tag" title={`Synced to Tempo · id ${block.tempo_worklog_id}`}>
              <Check />
              synced
            </span>
          )}
          {dirty && (
            <span
              className="dirty-tag"
              title={`Edited since last sync — click "Sync to Tempo" to update Tempo entry ${block.tempo_worklog_id}`}
            >
              unsynced edits
            </span>
          )}
          {block.is_personal && (
            <span
              className="personal-tag"
              title="Personal — auto-classified from project path. Skipped by estimator and Tempo sync."
            >
              personal
            </span>
          )}
        </div>

        <div
          ref={descRef}
          className={`block-description ${!block.description ? "empty" : ""}`}
          contentEditable={!isPending}
          suppressContentEditableWarning
          spellCheck={false}
          role="textbox"
          aria-multiline="true"
          aria-label="Block description — click to edit"
          aria-busy={isPending || undefined}
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
          <EventList blockId={block.id} eventCount={block.event_count} />
          <CommitList blockId={block.id} isPersonal={block.is_personal} />
        </div>
      </div>

      <div className="block-actions">
        <button
          type="button"
          className={`icon-btn personal-toggle${block.is_personal ? " active" : ""}`}
          title={
            block.is_personal
              ? "Personal — click to mark as work"
              : "Mark as personal (skips estimate + Tempo sync)"
          }
          aria-label={
            block.is_personal
              ? `mark ${timeRangeLabel} block as work`
              : `mark ${timeRangeLabel} block as personal`
          }
          aria-pressed={block.is_personal}
          aria-busy={isPending || undefined}
          onClick={onTogglePersonal}
        >
          <Coffee />
        </button>
        <button
          type="button"
          className="icon-btn danger"
          title="Delete block"
          aria-label={`delete ${timeRangeLabel} block`}
          aria-busy={isPending || undefined}
          onClick={onDelete}
        >
          <Trash2 />
        </button>
      </div>
    </article>
  );
}
