"use client";

// Filed (label_origin set) browser/Slack events: one collapsed-by-default
// disclosure, one line per event when expanded. Native <details> handles
// the collapse/chevron-rotate with pure CSS — no client state needed.

import { ChevronRight, Globe, Slack } from "lucide-react";
import type { LabelOrigin, RoutedEvent } from "@/lib/types";
import { formatEventTime } from "@/lib/format-event";

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

function AutoFiledRow({ event }: { event: RoutedEvent }) {
  return (
    <li className="auto-filed-row" data-source={event.source}>
      <span className="auto-filed-time">{formatEventTime(event.started_at)}</span>
      {event.source === "firefox" && <Globe width={13} height={13} strokeWidth={1.75} />}
      {event.source === "slack" && <Slack width={13} height={13} strokeWidth={1.75} />}
      <span className="auto-filed-title" title={event.title}>
        {event.title}
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
function originText(origin: LabelOrigin, folder: string | null): string {
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
      return "not work";
  }
}
