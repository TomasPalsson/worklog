// Turns a block's raw events into rows a person can read: your prompts
// (with a snippet), one row per stretch of Claude working (branch, tools,
// files it edited), shell commands folded into one line, and everything
// else as title + gist. Exact repeats (a session copied into two
// transcript files) are dropped.

import { eventGist, formatEventTime } from "./format-event";
import type { Event } from "./types";

export type RowKind = "prompt" | "claude" | "shell" | "git" | "github" | "slack" | "web" | "meeting" | "other";

export interface EventRow {
  key: string;
  kind: RowKind;
  time: string;
  text: string;
  detail: string;
}

/** A pause longer than this starts a new Claude/shell stretch. */
const STRETCH_GAP_MIN = 20;

const hhmm = formatEventTime;
const minutesOf = (iso: string) => Math.floor(new Date(iso).getTime() / 60_000);

function kindOf(source: string): RowKind {
  switch (source) {
    case "claude_turn":
      return "prompt";
    case "claude_work":
    case "claude":
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

/** Merge "branch X · Bash ×2, Edit · edited a, b" summaries into one. */
function mergeClaudeDetails(details: (string | null)[]): string {
  const branches = new Set<string>();
  const tools = new Map<string, number>();
  const files = new Set<string>();
  for (const d of details) {
    for (const part of (d ?? "").split(" · ")) {
      if (part.startsWith("branch ")) branches.add(part.slice(7));
      else if (part.startsWith("edited ")) part.slice(7).split(", ").forEach((f) => /^\+\d+$/.test(f) || files.add(f));
      else if (part)
        for (const t of part.split(", ")) {
          const [name, n] = t.split(" ×");
          tools.set(name, (tools.get(name) ?? 0) + (Number(n) || 1));
        }
    }
  }
  const toolText = [...tools]
    .sort((a, b) => b[1] - a[1])
    .map(([n, c]) => (c > 1 ? `${n} ×${c}` : n))
    .join(", ");
  const fileList = [...files];
  const fileText = fileList.length
    ? `edited ${fileList.slice(0, 4).join(", ")}${fileList.length > 4 ? ` +${fileList.length - 4}` : ""}`
    : "";
  return [branches.size ? `branch ${[...branches].join(", ")}` : "", toolText, fileText].filter(Boolean).join(" · ");
}

function countNames(titles: string[]): string {
  const counts = new Map<string, number>();
  titles.forEach((t) => counts.set(t, (counts.get(t) ?? 0) + 1));
  return [...counts].map(([t, c]) => (c > 1 ? `${t} ×${c}` : t)).join(", ");
}

export function eventRows(events: Event[]): EventRow[] {
  const seen = new Set<string>();
  const sorted = [...events]
    .sort((a, b) => a.started_at.localeCompare(b.started_at))
    .filter((e) => {
      const k = `${e.source}|${minutesOf(e.started_at)}|${e.title}|${e.details ?? ""}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });

  const rows: EventRow[] = [];
  let run: Event[] = [];
  const flush = () => {
    if (run.length === 0) return;
    const first = run[0];
    const last = run[run.length - 1];
    const kind = kindOf(first.source);
    const minutes = new Set(run.map((e) => minutesOf(e.started_at))).size;
    rows.push({
      key: `${first.id}`,
      kind,
      time: run.length > 1 ? `${hhmm(first.started_at)}–${hhmm(new Date(new Date(last.started_at).getTime() + 60_000).toISOString())}` : hhmm(first.started_at),
      text: kind === "claude" ? `Claude working · ${minutes} min` : `Ran ${countNames(run.map((e) => e.title))}`,
      detail: kind === "claude" ? mergeClaudeDetails(run.map((e) => e.details)) : "",
    });
    run = [];
  };

  for (const e of sorted) {
    const kind = kindOf(e.source);
    if (kind === "claude" || kind === "shell") {
      const prev = run[run.length - 1];
      const sameStretch =
        prev && kindOf(prev.source) === kind && minutesOf(e.started_at) - minutesOf(prev.started_at) <= STRETCH_GAP_MIN;
      if (!sameStretch) flush();
      run.push(e);
      continue;
    }
    flush();
    rows.push({
      key: `${e.id}`,
      kind,
      time: hhmm(e.started_at),
      text: kind === "prompt" ? (e.snippet ?? "You wrote a prompt") : e.title,
      detail: kind === "prompt" ? "" : eventGist(e.source, e.details),
    });
  }
  flush();
  return rows;
}
