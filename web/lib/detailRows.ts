// Folds a block's flat DetailRow list into the Details view timeline
// (design §2, §4, D-11): Claude sessions fold into prompt → tool calls →
// files touched, with helper/subagent activity and session messages
// nested under their parent session; everything else stays a plain
// event row. Pure functions, no React.

import type { DetailRow, HelperKind } from "./clues_contract";
import { SOURCE_CLAUDE_HELPER, SOURCE_CLAUDE_MESSAGE, SOURCE_CLAUDE_TOOL } from "./clues_contract";

export type SourceKind = "claude" | "shell" | "git" | "github" | "slack" | "web" | "meeting" | "other";

const CLAUDE_SOURCES = new Set<string>([
  "claude_turn",
  "claude_work",
  "claude",
  SOURCE_CLAUDE_TOOL,
  SOURCE_CLAUDE_HELPER,
  SOURCE_CLAUDE_MESSAGE,
]);

function kindOf(source: string): SourceKind {
  switch (source) {
    case "claude_turn":
    case "claude_work":
    case "claude":
    case SOURCE_CLAUDE_TOOL:
    case SOURCE_CLAUDE_HELPER:
    case SOURCE_CLAUDE_MESSAGE:
      return "claude";
    case "shell":
      return "shell";
    case "git_reflog":
      return "git";
    case "github_commit":
    case "github_pr":
      return "github";
    case "slack":
      return "slack";
    case "firefox":
      return "web";
    case "gcal":
      return "meeting";
    default:
      return "other";
  }
}

export interface Turn {
  prompt: DetailRow | null;
  tools: DetailRow[];
  files: string[];
}

export interface HelperGroup {
  title: string;
  helperKind: HelperKind | null;
  rows: DetailRow[];
}

export type TimelineItem =
  | { kind: "event"; source: SourceKind; row: DetailRow }
  | {
      kind: "session";
      sessionId: string;
      start: string;
      end: string;
      turns: Turn[];
      helpers: HelperGroup[];
      messages: DetailRow[];
      work: DetailRow[];
    };

/** Deduped union of a turn's tool `raw.files`, in first-seen order. */
function filesOf(tools: DetailRow[]): string[] {
  const seen = new Set<string>();
  const files: string[] = [];
  for (const tool of tools) {
    if (tool.raw?.kind !== "claude_tool") continue;
    for (const f of tool.raw.files) {
      if (!seen.has(f)) {
        seen.add(f);
        files.push(f);
      }
    }
  }
  return files;
}

/** One HelperGroup per title, in first-seen order. */
function groupHelpers(rows: DetailRow[]): HelperGroup[] {
  const order: string[] = [];
  const byTitle = new Map<string, DetailRow[]>();
  for (const row of rows) {
    if (!byTitle.has(row.title)) {
      byTitle.set(row.title, []);
      order.push(row.title);
    }
    byTitle.get(row.title)!.push(row);
  }
  return order.map((title) => {
    const groupRows = byTitle.get(title)!;
    const raw = groupRows[0].raw;
    return { title, helperKind: raw?.kind === "helper" ? raw.helper_kind : null, rows: groupRows };
  });
}

function buildSession(sessionId: string, rows: DetailRow[]): TimelineItem {
  const turns: Turn[] = [];
  let current: { prompt: DetailRow | null; tools: DetailRow[] } | null = null;
  const helperRows: DetailRow[] = [];
  const messages: DetailRow[] = [];
  const work: DetailRow[] = [];

  const flush = () => {
    if (current) turns.push({ prompt: current.prompt, tools: current.tools, files: filesOf(current.tools) });
  };

  for (const row of rows) {
    switch (row.source) {
      case "claude_turn":
        flush();
        current = { prompt: row, tools: [] };
        break;
      case SOURCE_CLAUDE_TOOL:
        if (!current) current = { prompt: null, tools: [] };
        current.tools.push(row);
        break;
      case SOURCE_CLAUDE_HELPER:
        helperRows.push(row);
        break;
      case SOURCE_CLAUDE_MESSAGE:
        messages.push(row);
        break;
      default: // claude_work, claude — the session summary
        work.push(row);
    }
  }
  flush();

  return {
    kind: "session",
    sessionId,
    start: rows[0].started_at,
    end: rows[rows.length - 1].started_at,
    turns,
    helpers: groupHelpers(helperRows),
    messages,
    work,
  };
}

/** Folds `rows` into timeline items, sessions positioned at their first
 * row's time; nothing is dropped. */
export function buildTimeline(rows: DetailRow[]): TimelineItem[] {
  const sessionOrder: string[] = [];
  const sessionRows = new Map<string, DetailRow[]>();
  const positioned: { time: string; item: TimelineItem }[] = [];

  for (const row of rows) {
    if (!CLAUDE_SOURCES.has(row.source)) {
      positioned.push({ time: row.started_at, item: { kind: "event", source: kindOf(row.source), row } });
      continue;
    }
    const sid = row.session_id ?? "unknown";
    if (!sessionRows.has(sid)) {
      sessionRows.set(sid, []);
      sessionOrder.push(sid);
    }
    sessionRows.get(sid)!.push(row);
  }

  for (const sid of sessionOrder) {
    const srows = sessionRows.get(sid)!;
    positioned.push({ time: srows[0].started_at, item: buildSession(sid, srows) });
  }

  positioned.sort((a, b) => a.time.localeCompare(b.time));
  return positioned.map((p) => p.item);
}

/** Only items whose source is enabled; session items count as "claude". */
export function filterTimeline(items: TimelineItem[], enabled: ReadonlySet<SourceKind>): TimelineItem[] {
  return items.filter((item) => enabled.has(item.kind === "session" ? "claude" : item.source));
}

const SOURCE_ORDER: SourceKind[] = ["claude", "shell", "git", "github", "slack", "web", "meeting", "other"];

/** Sources present in `items`, for filter chips, in a fixed order. */
export function sourcesPresent(items: TimelineItem[]): SourceKind[] {
  const present = new Set<SourceKind>();
  for (const item of items) present.add(item.kind === "session" ? "claude" : item.source);
  return SOURCE_ORDER.filter((k) => present.has(k));
}
