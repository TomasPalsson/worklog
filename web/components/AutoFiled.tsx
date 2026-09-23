"use client";

// Filed (label_origin set) browser/Slack events: one collapsed-by-default
// disclosure, one line per event when expanded. Native <details> handles
// the collapse/chevron-rotate with pure CSS — no client state needed.

import { ChevronRight, Globe, Slack } from "lucide-react";
import type { LabelOrigin, RoutedEvent } from "@/lib/types";
import { eventGist, formatEventTime } from "@/lib/format-event";

interface Props {
  events: RoutedEvent[];
}

export function AutoFiled({ events }: Props) {
  if (events.length === 0) return null;
  return (
    <details className="auto-filed">
      <summary>
        <ChevronRight className="disclosure-chev" />
        Auto-filed · {events.length}
      </summary>
      <ul className="auto-filed-list" role="list">
        {events.map((e) => (
          <AutoFiledRow key={e.id} event={e} />
        ))}
      </ul>
    </details>
  );
}

/** Exported so the review drawer's "Filed" section can reuse the exact
 * same row markup instead of a second lookalike. */
export function AutoFiledRow({ event }: { event: RoutedEvent }) {
  return (
    <li className="auto-filed-row" data-source={event.source}>
      <span className="auto-filed-time">{formatEventTime(event.started_at)}</span>
      {event.source === "firefox" && <Globe width={13} height={13} strokeWidth={1.75} />}
      {event.source === "slack" && <Slack width={13} height={13} strokeWidth={1.75} />}
      <span className="event-what">
        <span className="auto-filed-title" title={event.title}>
          {event.title}
        </span>
        <EventGist event={event} />
      </span>
      <span aria-hidden="true">→</span>
      <span className="auto-filed-project">{event.folder}</span>
      {event.label_origin && (
        <span className={`origin-chip origin-${event.label_origin}`}>
          {originText(event.label_origin, event.folder)}
        </span>
      )}
    </li>
  );
}

/** Plain-language reason text — the owner never wrote a "rule", so the
 * chip never claims they did. Mirrors the old UnsortedList badge copy. */
export function originText(origin: LabelOrigin, folder: string | null): string {
  switch (origin) {
    case "rule":
      return "your rule";
    case "link":
      return "names the repo";
    case "context":
      return `you were in ${folder ?? "this folder"} then`;
    case "guess":
      return "model guess";
    case "fix":
      return "you sorted";
    case "dismissed":
      return "you said not work";
    case "noise":
      return "no work nearby";
  }
}

/** The second line of a clue row: what the message said or where the visit
 * went, so a row is never just a name. Full text on hover. */
export function EventGist({ event }: { event: RoutedEvent }) {
  const gist = eventGist(event.source, event.details);
  if (!gist) return null;
  return (
    <span className="event-gist" title={event.details ?? undefined}>
      {gist}
    </span>
  );
}
