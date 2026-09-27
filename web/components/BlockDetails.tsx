"use client";

// Details view (spec 006, FR-19..FR-25, FR-34): filter chips over
// `sourcesPresent`, a timeline folding Claude sessions into
// prompt -> tools -> files with helper/message activity nested inside,
// every row expandable to its stored JSON.

import { useId, useMemo, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import type { DetailRow } from "@/lib/clues_contract";
import {
  buildTimeline,
  filterTimeline,
  sourcesPresent,
  type HelperGroup,
  type SourceKind,
  type TimelineItem,
  type Turn,
} from "@/lib/detailRows";
import { formatEventTime, previewDetails } from "@/lib/format-event";

const SOURCE_LABELS: Record<SourceKind, string> = {
  claude: "Claude",
  shell: "shell",
  git: "git",
  github: "GitHub",
  slack: "Slack",
  web: "web",
  meeting: "Calendar",
  other: "other",
};

interface Props {
  rows: DetailRow[];
}

export function BlockDetails({ rows }: Props) {
  const timeline = useMemo(() => buildTimeline(rows), [rows]);
  const sources = useMemo(() => sourcesPresent(timeline), [timeline]);
  const [enabled, setEnabled] = useState<Set<SourceKind>>(() => new Set(sources));
  const filtered = filterTimeline(timeline, enabled);

  const toggle = (source: SourceKind) => {
    setEnabled((prev) => {
      const next = new Set(prev);
      if (next.has(source)) next.delete(source);
      else next.add(source);
      return next;
    });
  };

  return (
    <div className="block-details">
      <div className="block-details-filters" role="group" aria-label="filter by source">
        {sources.map((source) => (
          <button
            key={source}
            type="button"
            className="block-details-chip"
            aria-pressed={enabled.has(source)}
            onClick={() => toggle(source)}
          >
            {SOURCE_LABELS[source]}
          </button>
        ))}
      </div>
      <div className="block-details-timeline">
        {filtered.map((item) =>
          item.kind === "event" ? (
            <EventRow key={item.row.id} item={item} />
          ) : (
            <SessionGroup key={item.sessionId} item={item} />
          ),
        )}
      </div>
    </div>
  );
}

/** One row's toggle to its full stored record (FR-24): a small button
 * beside the row's summary content, revealing `<pre>` JSON below. Kept
 * separate from `children` so an interactive child (the commit link)
 * never nests inside the toggle button itself. */
function RawToggle({
  row,
  label,
  className,
  children,
}: {
  row: unknown;
  label: string;
  className?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div className={className}>
      <div className="block-details-row-main">
        {children}
        <button
          type="button"
          className="block-details-expand"
          aria-expanded={open}
          aria-controls={id}
          aria-label={`${open ? "Hide" : "Show"} raw record — ${label}`}
          onClick={() => setOpen((v) => !v)}
        >
          <ChevronRight className={`disclosure-chev ${open ? "open" : ""}`} />
        </button>
      </div>
      {open && (
        <pre className="block-details-raw" id={id}>
          {JSON.stringify(row, null, 2)}
        </pre>
      )}
    </div>
  );
}

/** `https://github.com/<repo>/commit/<sha>` for a github_commit row with
 * both a repo and a non-empty sha (FR-34); null otherwise. */
function commitLink(row: DetailRow): string | null {
  if (row.source !== "github_commit" || row.raw?.kind !== "commit" || !row.repo) return null;
  return row.raw.sha ? `https://github.com/${row.repo}/commit/${row.raw.sha}` : null;
}

function EventRow({ item }: { item: Extract<TimelineItem, { kind: "event" }> }) {
  const { row, source } = item;
  const link = commitLink(row);
  return (
    <RawToggle row={row} label={row.title} className="block-details-row">
      <time className="block-details-time">{formatEventTime(row.started_at)}</time>
      <span className="block-details-source">{SOURCE_LABELS[source]}</span>
      <span className="block-details-title">
        {link ? (
          <a href={link} target="_blank" rel="noreferrer">
            {row.title}
          </a>
        ) : (
          row.title
        )}
      </span>
    </RawToggle>
  );
}

/** One-line preview of a tool's `input`, truncated like an event detail. */
function previewInput(input: unknown): string {
  if (input === null || input === undefined) return "";
  const json = typeof input === "string" ? input : JSON.stringify(input);
  return previewDetails(json, 100).preview;
}

function ToolRow({ tool }: { tool: DetailRow }) {
  const name = tool.raw?.kind === "claude_tool" ? tool.raw.tool : tool.title;
  const preview = tool.raw?.kind === "claude_tool" ? previewInput(tool.raw.input) : "";
  return (
    <RawToggle row={tool} label={name} className="block-details-row block-details-tool">
      <span className="block-details-tool-name">{name}</span>
      {preview && <span className="block-details-tool-preview">{preview}</span>}
    </RawToggle>
  );
}

function TurnBlock({ turn }: { turn: Turn }) {
  const promptText = turn.prompt
    ? turn.prompt.raw?.kind === "claude_prompt"
      ? turn.prompt.raw.text
      : turn.prompt.title
    : "(before first prompt)";
  return (
    <div className="block-details-turn">
      {turn.prompt ? (
        <RawToggle row={turn.prompt} label={promptText} className="block-details-row">
          <p className="block-details-prompt">{promptText}</p>
        </RawToggle>
      ) : (
        <p className="block-details-prompt block-details-prompt-empty">{promptText}</p>
      )}
      {turn.tools.map((tool) => (
        <ToolRow key={tool.id} tool={tool} />
      ))}
      {turn.files.length > 0 && (
        <p className="block-details-files">Files: {turn.files.join(", ")}</p>
      )}
    </div>
  );
}

function HelperGroupBlock({ helper }: { helper: HelperGroup }) {
  return (
    <details className="block-details-helper">
      <summary>
        {helper.title} · {helper.rows.length}
      </summary>
      <div className="block-details-helper-body">
        {helper.rows.map((row) => (
          <RawToggle
            key={row.id}
            row={row}
            label={row.raw?.kind === "helper" ? row.raw.summary : row.title}
            className="block-details-row"
          >
            <span className="block-details-title">
              {row.raw?.kind === "helper" ? row.raw.summary : row.title}
            </span>
          </RawToggle>
        ))}
      </div>
    </details>
  );
}

function MessageRow({ row }: { row: DetailRow }) {
  const from = row.raw?.kind === "session_message" ? row.raw.from : "unknown";
  return (
    <RawToggle row={row} label={`message from ${from}`} className="block-details-row">
      <span className="block-details-title">message from {from}</span>
    </RawToggle>
  );
}

function SessionGroup({ item }: { item: Extract<TimelineItem, { kind: "session" }> }) {
  const range = `${formatEventTime(item.start)}–${formatEventTime(item.end)}`;
  return (
    <details className="block-details-session">
      <summary>
        {range} · Claude session
      </summary>
      <div className="block-details-session-body">
        {item.turns.map((turn, i) => (
          <TurnBlock key={i} turn={turn} />
        ))}
        {item.helpers.map((helper, i) => (
          <HelperGroupBlock key={i} helper={helper} />
        ))}
        {item.messages.map((row) => (
          <MessageRow key={row.id} row={row} />
        ))}
        {item.work.map((row) => (
          <RawToggle key={row.id} row={row} label={row.title} className="block-details-row">
            <span className="block-details-title">{row.title}</span>
          </RawToggle>
        ))}
      </div>
    </details>
  );
}
