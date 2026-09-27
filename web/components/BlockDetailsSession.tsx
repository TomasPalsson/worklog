"use client";

// A Claude session on the Details timeline: a card whose closed state
// already says what happened (counts + the first prompt), opening into each
// prompt with the tool calls it led to and the files they touched, then the
// subagent / background work and messages that belonged to it.

import { useState } from "react";
import { ChevronDown, FileCode2, Users } from "lucide-react";
import type { DetailRow } from "@/lib/clues_contract";
import type { HelperGroup, TimelineItem, Turn } from "@/lib/detailRows";
import { basename, toolPreview } from "@/lib/detailText";
import { formatEventTime } from "@/lib/format-event";
import { formatDuration } from "@/lib/format";
import { RowFull } from "./BlockDetails";
import { ClaudeMark, ToolIcon } from "./SourceIcon";

type Session = Extract<TimelineItem, { kind: "session" }>;

/** Tool calls shown per prompt before "Show all". */
const TOOLS_SHOWN = 6;
/** Files shown per prompt before "+N more". */
const FILES_SHOWN = 10;
/** Past this many characters a prompt is clamped and offers "Show full prompt". */
const LONG_PROMPT = 280;

function promptText(row: DetailRow): string {
  return row.raw?.kind === "claude_prompt" ? row.raw.text : "Prompt text not captured (collected before full capture)";
}

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

function sessionStats(item: Session): string {
  const prompts = item.turns.filter((t) => t.prompt).length;
  const tools = item.turns.reduce((n, t) => n + t.tools.length, 0);
  const files = new Set(item.turns.flatMap((t) => t.files)).size;
  return [
    prompts > 0 && plural(prompts, "prompt"),
    tools > 0 && plural(tools, "tool call"),
    files > 0 && plural(files, "file"),
    item.helpers.length > 0 && plural(item.helpers.length, "helper"),
  ]
    .filter(Boolean)
    .join(" · ");
}

export function SessionCard({ item, defaultOpen }: { item: Session; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const first = item.turns.find((t) => t.prompt)?.prompt;
  const seconds = (Date.parse(item.end) - Date.parse(item.start)) / 1000;
  return (
    <li className="bd-row" data-kind="claude">
      <time className="bd-time">{formatEventTime(item.start)}</time>
      <span className="bd-icon" aria-hidden="true">
        <ClaudeMark size={13} />
      </span>
      <div className="bd-body">
        <div className="bd-session" data-open={open}>
          <button type="button" className="bd-session-head" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
            <span className="bd-session-title">
              Claude session
              <span className="bd-session-when">
                {formatEventTime(item.start)}–{formatEventTime(item.end)} · {formatDuration(seconds)}
              </span>
            </span>
            <span className="bd-session-stats">{sessionStats(item)}</span>
            {!open && first && <span className="bd-session-preview">{promptText(first)}</span>}
            <ChevronDown className="bd-chev" width={16} height={16} aria-hidden="true" />
          </button>
          {open && <SessionBody item={item} />}
        </div>
      </div>
    </li>
  );
}

function SessionBody({ item }: { item: Session }) {
  return (
    <div className="bd-session-body">
      {item.turns.map((turn, i) => (
        <TurnView key={turn.prompt?.id ?? `t${i}`} turn={turn} />
      ))}
      {item.helpers.length > 0 && (
        <section className="bd-section" aria-label="Helpers">
          <h4 className="bd-section-title">Subagents and background work</h4>
          {item.helpers.map((helper) => (
            <HelperView key={helper.title} helper={helper} />
          ))}
        </section>
      )}
      {item.messages.length > 0 && (
        <section className="bd-section" aria-label="Messages">
          <h4 className="bd-section-title">Messages from other sessions</h4>
          {item.messages.map((row) => (
            <MessageView key={row.id} row={row} />
          ))}
        </section>
      )}
      {item.work.length > 0 && (
        <details className="bd-markers">
          <summary>{plural(item.work.length, "activity marker")}</summary>
          {item.work.map((row) => (
            <p key={row.id} className="bd-marker">
              <time>{formatEventTime(row.started_at)}</time> {row.details ?? row.title}
            </p>
          ))}
        </details>
      )}
    </div>
  );
}

function TurnView({ turn }: { turn: Turn }) {
  const [allTools, setAllTools] = useState(false);
  const tools = allTools ? turn.tools : turn.tools.slice(0, TOOLS_SHOWN);
  return (
    <div className="bd-turn">
      {turn.prompt ? <PromptView row={turn.prompt} /> : <p className="bd-turn-lead">Before the first prompt</p>}
      {tools.length > 0 && (
        <ul className="bd-tools">
          {tools.map((tool) => (
            <ToolRow key={tool.id} tool={tool} />
          ))}
        </ul>
      )}
      {turn.tools.length > TOOLS_SHOWN && (
        <button type="button" className="bd-more" onClick={() => setAllTools((v) => !v)}>
          {allTools ? "Show fewer" : `Show all ${turn.tools.length} tool calls`}
        </button>
      )}
      {turn.files.length > 0 && <FileChips files={turn.files} />}
    </div>
  );
}

function FileChips({ files }: { files: string[] }) {
  const [all, setAll] = useState(false);
  const shown = all ? files : files.slice(0, FILES_SHOWN);
  return (
    <ul className="bd-files" aria-label="Files touched">
      {shown.map((file) => (
        <li key={file} className="bd-file" title={file}>
          <FileCode2 width={12} height={12} aria-hidden="true" />
          {basename(file)}
        </li>
      ))}
      {files.length > FILES_SHOWN && (
        <li>
          <button type="button" className="bd-more" onClick={() => setAll((v) => !v)}>
            {all ? "Show fewer" : `+${files.length - FILES_SHOWN} more`}
          </button>
        </li>
      )}
    </ul>
  );
}

function PromptView({ row }: { row: DetailRow }) {
  const [open, setOpen] = useState(false);
  const text = promptText(row);
  return (
    <div className="bd-prompt">
      <div className="bd-prompt-meta">
        <span>You</span>
        <time>{formatEventTime(row.started_at)}</time>
      </div>
      <p className={open ? "bd-prompt-text" : "bd-prompt-text bd-clamp"}>{text}</p>
      <button type="button" className="bd-more" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        {open ? "Hide" : text.length > LONG_PROMPT ? "Show full prompt" : "Details"}
      </button>
      {open && <RowFull row={row} text={{ text, detail: null, mono: false }} />}
    </div>
  );
}

function ToolRow({ tool }: { tool: DetailRow }) {
  const [open, setOpen] = useState(false);
  const raw = tool.raw?.kind === "claude_tool" ? tool.raw : null;
  const name = raw?.tool ?? tool.title;
  const preview = raw ? toolPreview(raw.tool, raw.input) : "";
  const output = raw?.output
    ? `${raw.output}${raw.output_cut_bytes > 0 ? `\n… ${raw.output_cut_bytes} more bytes not kept` : ""}`
    : null;
  return (
    <li className="bd-tool">
      <button type="button" className="bd-tool-line" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        <span className="bd-tool-name">
          <ToolIcon tool={name} />
          {name}
        </span>
        <span className="bd-tool-preview">{preview}</span>
        <time className="bd-tool-time">{formatEventTime(tool.started_at)}</time>
      </button>
      {open && <RowFull row={tool} text={{ text: output ?? (preview || name), detail: null, mono: true }} />}
    </li>
  );
}

function HelperView({ helper }: { helper: HelperGroup }) {
  const [open, setOpen] = useState(false);
  const kind = helper.helperKind?.replace("_", " ") ?? "helper";
  return (
    <div className="bd-helper">
      <button type="button" className="bd-helper-line" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        <Users width={13} height={13} aria-hidden="true" />
        <span className="bd-helper-title">{helper.title}</span>
        <span className="bd-helper-meta">
          {kind} · {plural(helper.rows.length, "min")}
        </span>
      </button>
      {open && (
        <ul className="bd-helper-rows">
          {helper.rows.map((row) => (
            <li key={row.id}>
              <time>{formatEventTime(row.started_at)}</time>
              <span>{row.raw?.kind === "helper" ? row.raw.summary || "working" : row.title}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function MessageView({ row }: { row: DetailRow }) {
  const [open, setOpen] = useState(false);
  const raw = row.raw?.kind === "session_message" ? row.raw : null;
  return (
    <div className="bd-helper">
      <button type="button" className="bd-helper-line" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        <span className="bd-helper-title">From {raw?.from ?? "another session"}</span>
        <time className="bd-helper-meta">{formatEventTime(row.started_at)}</time>
      </button>
      {open && <RowFull row={row} text={{ text: raw?.text ?? row.title, detail: null, mono: false }} />}
    </div>
  );
}
