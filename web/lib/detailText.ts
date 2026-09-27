// What a Details-view row says: the stored raw record turned into one
// readable line (`text`) plus a quieter second line (`detail`). Pure, so the
// wording is unit-tested apart from the React tree.

import type { DetailRow } from "./clues_contract";

export interface RowText {
  text: string;
  detail: string | null;
  /** Render `text` in the mono face (commands, tool arguments). */
  mono: boolean;
}

const HOME_RE = /^\/Users\/[^/]+/;

/** `/Users/me/Desktop/Work/x` → `~/Desktop/Work/x`. */
export function shortPath(path: string): string {
  return path.replace(HOME_RE, "~");
}

export function basename(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function hostOf(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).host || null;
  } catch {
    return null;
  }
}

/** A reflog message (`commit: fix x`, `checkout: moving from a to b`, …) as
 * the change it made, with the git verb as the quiet detail. */
export function reflogText(message: string): RowText {
  const colon = message.indexOf(": ");
  const verb = colon > 0 ? message.slice(0, colon) : message;
  const rest = colon > 0 ? message.slice(colon + 2) : "";
  const checkout = rest.match(/^moving from (\S+) to (\S+)$/);
  if (verb === "checkout" && checkout) {
    return { text: `Switched to ${checkout[2]}`, detail: `from ${checkout[1]}`, mono: false };
  }
  if (verb.startsWith("merge ")) {
    return { text: `Merged ${verb.slice(6)}`, detail: rest || null, mono: false };
  }
  return { text: rest || verb, detail: rest ? verb : null, mono: false };
}

/** Slack's `<url|label>` / `<url>` / `<@U123>` markup as plain text. */
export function slackPlain(text: string): string {
  return text
    .replace(/<([^|>]+)\|([^>]+)>/g, "$2")
    .replace(/<(https?:[^>]+)>/g, "$1")
    .replace(/<[@#!][^>]*>/g, "@…");
}

/** Slack channel names are lower-case slugs; anything else is a person (a DM). */
function slackText(row: DetailRow): RowText {
  const isChannel = /^[a-z0-9][a-z0-9._-]*$/.test(row.title);
  return {
    text: isChannel ? `#${row.title}` : `${row.title} · direct message`,
    detail: row.details ? slackPlain(row.details) : null,
    mono: false,
  };
}

/** Rows collected before full capture carry only a bare verb as title. */
const BARE_TITLES: Record<string, string> = {
  git_reflog: "message not captured",
  claude_turn: "prompt text not captured",
  shell: "full command not captured",
};

export function rowText(row: DetailRow): RowText {
  const raw = row.raw;
  if (raw?.kind === "shell") {
    return { text: raw.command, detail: raw.cwd ? shortPath(raw.cwd) : null, mono: true };
  }
  if (raw?.kind === "reflog") return reflogText(raw.message);
  if (row.source === "slack") return slackText(row);
  if (row.source === "firefox") {
    return { text: row.title, detail: hostOf(row.details), mono: false };
  }
  if (row.source === "github_commit" || row.source === "github_pr") {
    return { text: row.title, detail: row.repo, mono: false };
  }
  const missing = raw === null ? BARE_TITLES[row.source] : undefined;
  return { text: row.title, detail: missing ? `${missing} (collected before full capture)` : row.details, mono: false };
}

function field(input: unknown, key: string): string | null {
  if (!input || typeof input !== "object") return null;
  const value = (input as Record<string, unknown>)[key];
  return typeof value === "string" && value ? value : null;
}

/** One line saying what a tool call touched: the command for Bash, the file
 * for Read/Edit/Write, the pattern for Grep/Glob, the task for Agent. */
export function toolPreview(tool: string, input: unknown): string {
  const path = field(input, "file_path") ?? field(input, "notebook_path") ?? field(input, "path");
  switch (tool) {
    case "Bash":
      return field(input, "command") ?? "";
    case "Read":
    case "Edit":
    case "MultiEdit":
    case "Write":
    case "NotebookEdit":
      return path ? shortPath(path) : "";
    case "Grep":
    case "Glob":
      return [field(input, "pattern"), path && shortPath(path)].filter(Boolean).join("  in  ");
    case "Agent":
    case "Task":
      return field(input, "description") ?? field(input, "prompt") ?? "";
    case "WebFetch":
      return hostOf(field(input, "url")) ?? "";
    case "WebSearch":
      return field(input, "query") ?? "";
    default: {
      if (input === null || input === undefined) return "";
      const json = typeof input === "string" ? input : JSON.stringify(input);
      return json.length > 140 ? `${json.slice(0, 140)}…` : json;
    }
  }
}

export interface Repeated<T> {
  item: T;
  count: number;
}

/** Collapse runs of rows that read the same (the same shell command twice in
 * a row) into one row with a count, so the timeline isn't a wall of
 * duplicates. Order is kept; only adjacent repeats fold. */
export function foldRepeats<T>(items: T[], keyOf: (item: T) => string): Repeated<T>[] {
  const out: Repeated<T>[] = [];
  for (const item of items) {
    const last = out[out.length - 1];
    if (last && keyOf(last.item) === keyOf(item)) last.count += 1;
    else out.push({ item, count: 1 });
  }
  return out;
}
