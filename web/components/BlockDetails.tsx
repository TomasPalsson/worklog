"use client";

// A block's full story on one timeline: filter chips (with counts) per
// source, Claude sessions as cards that open into prompt → tool calls →
// files, and every row opening in place to its full text and stored record.
// Same visual grammar as the inline "What happened" list (.ev-*): a time
// gutter, a round source icon, then the text.

import { useMemo, useState } from "react";
import { ChevronsDownUp, ChevronsUpDown, ExternalLink } from "lucide-react";
import type { DetailRow } from "@/lib/clues_contract";
import {
  buildTimeline,
  filterTimeline,
  sourcesPresent,
  type SourceKind,
  type TimelineItem,
} from "@/lib/detailRows";
import { foldRepeats, rowText, type RowText } from "@/lib/detailText";
import { formatEventTime } from "@/lib/format-event";
import { SessionCard } from "./BlockDetailsSession";
import { SOURCE_ICON } from "./SourceIcon";

export const SOURCE_LABEL: Record<SourceKind, string> = {
  claude: "Claude",
  shell: "Shell",
  git: "Git",
  github: "GitHub",
  slack: "Slack",
  web: "Web",
  meeting: "Calendar",
  other: "Other",
};

function itemRows(item: TimelineItem): number {
  if (item.kind === "event") return 1;
  const turns = item.turns.reduce((n, t) => n + t.tools.length + (t.prompt ? 1 : 0), 0);
  const helpers = item.helpers.reduce((n, h) => n + h.rows.length, 0);
  return turns + helpers + item.messages.length + item.work.length;
}

function countsBySource(items: TimelineItem[]): Map<SourceKind, number> {
  const counts = new Map<SourceKind, number>();
  for (const item of items) {
    const source = item.kind === "event" ? item.source : "claude";
    counts.set(source, (counts.get(source) ?? 0) + itemRows(item));
  }
  return counts;
}

function itemKey(item: TimelineItem): string {
  return item.kind === "event" ? `${item.source}|${rowText(item.row).text}` : `session|${item.sessionId}`;
}

export function BlockDetails({ rows }: { rows: DetailRow[] }) {
  const timeline = useMemo(() => buildTimeline(rows), [rows]);
  const sources = useMemo(() => sourcesPresent(timeline), [timeline]);
  const counts = useMemo(() => countsBySource(timeline), [timeline]);
  const [enabled, setEnabled] = useState<Set<SourceKind>>(() => new Set(sources));
  // null = each session keeps its own state; true/false = the last
  // "Expand all"/"Collapse all" (sessions remount with that as default).
  const [allOpen, setAllOpen] = useState<boolean | null>(null);
  const folded = foldRepeats(filterTimeline(timeline, enabled), itemKey);
  const hasSessions = timeline.some((item) => item.kind === "session");

  const toggle = (source: SourceKind) =>
    setEnabled((prev) => {
      const next = new Set(prev);
      if (next.has(source)) next.delete(source);
      else next.add(source);
      return next;
    });

  return (
    <div className="bd">
      <div className="bd-toolbar">
        <FilterChips sources={sources} counts={counts} enabled={enabled} onToggle={toggle} />
        {hasSessions && (
          <button type="button" className="bd-ghost" onClick={() => setAllOpen((v) => !v)}>
            {allOpen ? <ChevronsDownUp width={14} height={14} /> : <ChevronsUpDown width={14} height={14} />}
            {allOpen ? "Collapse all" : "Expand all"}
          </button>
        )}
      </div>

      {folded.length === 0 ? (
        <p className="bd-empty">Nothing to show — turn a source back on above.</p>
      ) : (
        <ol className="bd-timeline">
          {folded.map(({ item, count }) =>
            item.kind === "event" ? (
              <EventRow key={item.row.id} row={item.row} source={item.source} count={count} />
            ) : (
              <SessionCard key={`${item.sessionId}-${String(allOpen)}`} item={item} defaultOpen={allOpen ?? false} />
            ),
          )}
        </ol>
      )}
    </div>
  );
}

function FilterChips({
  sources,
  counts,
  enabled,
  onToggle,
}: {
  sources: SourceKind[];
  counts: Map<SourceKind, number>;
  enabled: Set<SourceKind>;
  onToggle: (source: SourceKind) => void;
}) {
  return (
    <div className="bd-filters" role="group" aria-label="Filter by source">
      {sources.map((source) => {
        const Icon = SOURCE_ICON[source];
        return (
          <button
            key={source}
            type="button"
            className="bd-chip"
            data-kind={source}
            aria-pressed={enabled.has(source)}
            onClick={() => onToggle(source)}
          >
            <Icon size={13} />
            {SOURCE_LABEL[source]}
            <span className="bd-chip-count">{counts.get(source) ?? 0}</span>
          </button>
        );
      })}
    </div>
  );
}

/** `https://github.com/<repo>/commit/<sha>` for a commit with both. */
function commitLink(row: DetailRow): string | null {
  if (row.source !== "github_commit" || row.raw?.kind !== "commit" || !row.repo || !row.raw.sha) return null;
  return `https://github.com/${row.repo}/commit/${row.raw.sha}`;
}

/** What an opened row shows: the whole text (nothing clamped), its detail,
 * and the stored record behind a disclosure. */
export function RowFull({ row, text }: { row: DetailRow; text: RowText }) {
  return (
    <div className="bd-full">
      <p className={text.mono ? "bd-full-text bd-mono" : "bd-full-text"}>{text.text}</p>
      {text.detail && <p className="bd-full-detail">{text.detail}</p>}
      <details className="bd-raw">
        <summary>Stored record</summary>
        <pre>{JSON.stringify(row, null, 2)}</pre>
      </details>
    </div>
  );
}

function EventRow({ row, source, count }: { row: DetailRow; source: SourceKind; count: number }) {
  const [open, setOpen] = useState(false);
  const text = rowText(row);
  const link = commitLink(row);
  const Icon = SOURCE_ICON[source];
  return (
    <li className="bd-row" data-kind={source}>
      <time className="bd-time">{formatEventTime(row.started_at)}</time>
      <span className="bd-icon" aria-hidden="true">
        <Icon size={13} />
      </span>
      <div className="bd-body">
        <div className="bd-line">
          <button type="button" className="bd-toggle" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
            <span className={text.mono ? "bd-text bd-mono" : "bd-text"}>{text.text}</span>
            {count > 1 && <span className="bd-repeat">×{count}</span>}
          </button>
          {link && (
            <a className="bd-link" href={link} target="_blank" rel="noreferrer">
              <ExternalLink width={12} height={12} aria-hidden="true" />
              Open on GitHub
            </a>
          )}
        </div>
        {open ? <RowFull row={row} text={text} /> : text.detail && <p className="bd-detail">{text.detail}</p>}
      </div>
    </li>
  );
}
