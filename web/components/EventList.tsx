"use client";

import { useState, useTransition } from "react";
import {
  Bot,
  Calendar,
  ChevronRight,
  Circle,
  GitBranch,
  GitCommit,
  Globe,
  MessageSquareText,
  Slack,
  Terminal,
} from "lucide-react";
import type { Event } from "@/lib/types";
import { eventRows, type EventRow, type RowKind } from "@/lib/eventRows";
import { fetchBlockEvents } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  blockId: number;
  eventCount: number;
}

/**
 * Per-block drill-down: a readable timeline of what happened — your
 * prompts (first words), each stretch of Claude working (branch, tools,
 * files), shell commands and the rest. Fetched once on first expand.
 */
export function EventList({ blockId, eventCount }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [events, setEvents] = useState<Event[] | null>(null);
  const [isPending, start] = useTransition();

  if (eventCount === 0) {
    return (
      <span className="events-disclosure empty" aria-disabled="true">
        <Circle width={11} height={11} />
        no events
      </span>
    );
  }

  const toggle = () => {
    if (!expanded && events === null) {
      start(async () => {
        const r = await fetchBlockEvents(blockId);
        if (r.ok) setEvents(r.data);
        else toast.error(`Load events failed — ${r.error}`);
      });
    }
    setExpanded((v) => !v);
  };

  return (
    <div className="events-disclosure-wrap">
      <button
        type="button"
        className="events-disclosure"
        aria-expanded={expanded}
        aria-controls={`events-list-${blockId}`}
        aria-busy={isPending || undefined}
        onClick={toggle}
      >
        <ChevronRight className={`disclosure-chev ${expanded ? "open" : ""}`} />
        {expanded ? "Hide what happened" : "What happened"}
      </button>

      {expanded && (
        <ol className="ev-timeline" id={`events-list-${blockId}`}>
          {events === null ? (
            <li className="events-loading" aria-live="polite">
              loading…
            </li>
          ) : (
            eventRows(events).map((row) => <TimelineRow key={row.key} row={row} />)
          )}
        </ol>
      )}
    </div>
  );
}

const ICONS: Record<RowKind, typeof Circle> = {
  prompt: MessageSquareText,
  claude: Bot,
  shell: Terminal,
  git: GitBranch,
  github: GitCommit,
  slack: Slack,
  web: Globe,
  meeting: Calendar,
  other: Circle,
};

function TimelineRow({ row }: { row: EventRow }) {
  const Icon = ICONS[row.kind];
  return (
    <li className="ev-row" data-kind={row.kind}>
      <time className="ev-time">{row.time}</time>
      <span className="ev-icon" aria-hidden="true">
        <Icon width={13} height={13} strokeWidth={1.75} />
      </span>
      <div className="ev-body">
        <p className={row.kind === "prompt" ? "ev-text ev-prompt" : "ev-text"}>{row.text}</p>
        {row.detail && <p className="ev-detail">{row.detail}</p>}
      </div>
    </li>
  );
}
