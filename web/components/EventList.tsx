"use client";

import { useState, useTransition } from "react";
import {
  ChevronRight,
  GitCommit,
  MessageSquare,
  Calendar,
  Briefcase,
  Circle,
} from "lucide-react";
import type { Event } from "@/lib/types";
import { sourceKind } from "@/lib/types";
import { formatEventTime, previewDetails, sourceLabel } from "@/lib/format-event";
import { fetchBlockEvents } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  blockId: number;
  eventCount: number;
}

/**
 * Per-block drill-down. Collapsed by default; clicking the disclosure
 * triggers a one-time fetch of the events via Server Action and keeps
 * the result cached on the component for subsequent toggles.
 *
 * Design note: the expanded list renders inline under the block meta
 * row. An alternative was a side drawer — rejected because it breaks
 * the user's reading position and creates a focus trap for keyboard
 * users. Progressive disclosure in-place keeps Jakob's Law on side
 * and costs no layout shift when collapsed.
 */
export function EventList({ blockId, eventCount }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [events, setEvents] = useState<Event[] | null>(null);
  const [isPending, start] = useTransition();

  // Zero-event blocks still get a disabled chip so the spot in the meta
  // row is visually consistent across all cards — but clicking does
  // nothing, and the ARIA state says so.
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
        if (r.ok) {
          setEvents(r.data);
        } else {
          toast.error(`Load events failed — ${r.error}`);
          return;
        }
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
        {eventCount} event{eventCount === 1 ? "" : "s"}
      </button>

      {expanded && (
        <ul
          className="events-list"
          id={`events-list-${blockId}`}
          role="list"
        >
          {events === null ? (
            <li className="events-loading" aria-live="polite">
              loading…
            </li>
          ) : (
            events.map((e) => <EventRow key={e.id} event={e} />)
          )}
        </ul>
      )}
    </div>
  );
}

function EventRow({ event }: { event: Event }) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const kind = sourceKind(event.source);
  const { preview, truncated } = previewDetails(event.details, 160);
  const time = formatEventTime(event.started_at);

  return (
    <li className="event-row" data-source={kind}>
      <span className="event-source" aria-label={sourceLabel(kind)} title={sourceLabel(kind)}>
        {iconFor(kind)}
      </span>
      <div className="event-main">
        <div className="event-head">
          <span className="event-title">{event.title}</span>
          <span className="event-time" aria-label={`at ${time}`}>
            {time}
          </span>
        </div>
        {preview && (
          <div className="event-preview">
            <span className={detailsOpen ? "preview-text open" : "preview-text"}>
              {detailsOpen ? event.details : preview}
            </span>
            {truncated && (
              <button
                type="button"
                className="event-expand"
                aria-expanded={detailsOpen}
                onClick={() => setDetailsOpen((v) => !v)}
              >
                {detailsOpen ? "Show less" : "Show more"}
              </button>
            )}
          </div>
        )}
      </div>
    </li>
  );
}

function iconFor(kind: ReturnType<typeof sourceKind>) {
  const size = 13;
  switch (kind) {
    case "github":
      return <GitCommit width={size} height={size} strokeWidth={1.75} />;
    case "claude":
      return <MessageSquare width={size} height={size} strokeWidth={1.75} />;
    case "gcal":
      return <Calendar width={size} height={size} strokeWidth={1.75} />;
    case "jira":
      return <Briefcase width={size} height={size} strokeWidth={1.75} />;
    default:
      return <Circle width={size} height={size} strokeWidth={1.75} />;
  }
}
