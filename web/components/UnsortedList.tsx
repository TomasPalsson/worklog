"use client";

// The day page's browser/Slack event feed (B12): every routed event shows
// its source and label origin, and the unsorted ones get an inline picker
// so a fix — optionally promoted to a hard "always" rule — is one click.

import { useState, useTransition } from "react";
import { Globe, Slack } from "lucide-react";
import { labelEvent } from "@/app/actions";
import { toast } from "@/lib/toast";
import { formatEventTime, previewDetails } from "@/lib/format-event";
import type { LabelOrigin, RoutedEvent, RuleKind } from "@/lib/types";
import { PalettePicker } from "./PalettePicker";

interface Props {
  day: string;
  events: RoutedEvent[];
  /** Candidate project keys offered by the picker (billing folders ∪
   * folders seen in events with no mapping yet). */
  folderOptions: string[];
}

export function UnsortedList({ day, events, folderOptions }: Props) {
  const [rows, setRows] = useState(events);
  if (rows.length === 0) return null;

  const unsorted = rows.filter((e) => e.folder === null);
  const labelled = rows.filter((e) => e.folder !== null);

  function handleLabelled(updated: RoutedEvent) {
    setRows((prev) => prev.map((e) => (e.id === updated.id ? updated : e)));
  }

  return (
    <section className="unsorted-section" aria-label="Browser and Slack events">
      <h2>Unsorted{unsorted.length > 0 ? ` (${unsorted.length})` : ""}</h2>
      {unsorted.length === 0 ? (
        <p className="unsorted-empty">Nothing unsorted today.</p>
      ) : (
        <ul className="unsorted-list" role="list">
          {unsorted.map((e) => (
            <UnsortedRow
              key={e.id}
              event={e}
              day={day}
              folderOptions={folderOptions}
              onLabelled={handleLabelled}
            />
          ))}
        </ul>
      )}
      {labelled.length > 0 && (
        <ul className="routed-list" role="list">
          {labelled.map((e) => (
            <RoutedRow key={e.id} event={e} />
          ))}
        </ul>
      )}
    </section>
  );
}

function ruleKindFor(source: string): RuleKind {
  return source === "slack" ? "slack_channel" : "domain";
}

function UnsortedRow({
  event,
  day,
  folderOptions,
  onLabelled,
}: {
  event: RoutedEvent;
  day: string;
  folderOptions: string[];
  onLabelled: (updated: RoutedEvent) => void;
}) {
  const [always, setAlways] = useState(false);
  const [pending, start] = useTransition();
  const [pickError, setPickError] = useState<string | null>(null);
  const ruleKind = ruleKindFor(event.source);
  const scope = ruleKind === "slack_channel" ? "channel" : "site";
  const { preview } = previewDetails(event.details, 120);

  function pick(folder: string) {
    setPickError(null);
    start(async () => {
      const r = await labelEvent(event.id, folder, always ? ruleKind : null, day);
      if (!r.ok) {
        setPickError(r.error);
        toast.error(`Couldn't label — ${r.error}`);
        return;
      }
      onLabelled(r.data);
    });
  }

  return (
    <li className="event-row unsorted-row" data-source={event.source}>
      <SourceTag event={event} />
      <div className="event-main">
        <div className="event-head">
          <span className="event-title">{event.title}</span>
          <span className="event-time">{formatEventTime(event.started_at)}</span>
        </div>
        {preview && <div className="event-preview">{preview}</div>}
      </div>
      <label className="unsorted-always">
        <input
          type="checkbox"
          checked={always}
          aria-label={`Always for this ${scope} — ${event.title}`}
          onChange={(e) => setAlways(e.target.checked)}
        />
        always for this {scope}
      </label>
      <PalettePicker
        value={null}
        options={folderOptions}
        allowFree
        placeholder="Pick project"
        searchPlaceholder="Search projects…"
        label={`Project for ${event.title}`}
        busy={pending}
        onPick={pick}
      />
      {pickError && (
        <p className="picker-error" role="alert">
          {pickError}
        </p>
      )}
    </li>
  );
}

function RoutedRow({ event }: { event: RoutedEvent }) {
  const { preview } = previewDetails(event.details, 120);
  return (
    <li className="event-row routed-row" data-source={event.source}>
      <SourceTag event={event} />
      <div className="event-main">
        <div className="event-head">
          <span className="event-title">{event.title}</span>
          <span className="event-time">{formatEventTime(event.started_at)}</span>
        </div>
        {preview && <div className="event-preview">{preview}</div>}
      </div>
      <span className="folder-label">{event.folder}</span>
      {event.label_origin && <OriginBadge origin={event.label_origin} folder={event.folder} />}
    </li>
  );
}

function SourceTag({ event }: { event: RoutedEvent }) {
  const isFirefox = event.source === "firefox";
  const isSlack = event.source === "slack";
  const label = isFirefox ? "Firefox" : isSlack ? "Slack" : event.source;
  const context = isFirefox ? event.container : null;
  return (
    <span className="event-source" title={context ? `${label} · ${context}` : label}>
      {isFirefox && <Globe width={13} height={13} strokeWidth={1.75} />}
      {isSlack && <Slack width={13} height={13} strokeWidth={1.75} />}
      <span className="event-source-label">
        {label}
        {context ? ` · ${context}` : ""}
      </span>
    </span>
  );
}

/** Plain-language badge text — the owner never wrote a "rule", so the badge
 * never claims they did. */
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
  }
}

function OriginBadge({ origin, folder }: { origin: LabelOrigin; folder: string | null }) {
  return <span className={`origin-badge origin-${origin}`}>{originText(origin, folder)}</span>;
}
