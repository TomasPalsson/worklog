"use client";

// Related issues and attachments under the description: parent, subtasks, linked issues, files.

import { useState, type ReactNode } from "react";
import { ExternalLink, Paperclip } from "lucide-react";

import { formatStamp } from "@/lib/taskBoard";
import type { Attachment, IssueRef, TicketDetail } from "@/lib/types";
import { TypeIcon } from "./TaskCardMeta";

const FIRST = 5;

export interface TaskRelatedProps {
  detail: TicketDetail | null;
  /** Keys on the board; these open in the modal instead of Jira. */
  knownKeys?: Set<string>;
  onOpen?: (key: string) => void;
}

/** Jira's base for `/browse/KEY` links, keeping a context path (`https://host/jira`); null when the url is not a ticket url. */
const jiraBase = (url: string): string | null => {
  const at = url.lastIndexOf("/browse/");
  return at > 0 ? url.slice(0, at) : null;
};

const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

type RowProps = { issue: IssueRef; base: string | null } & Pick<TaskRelatedProps, "knownKeys" | "onOpen">;

function IssueRow({ issue, base, knownKeys, onOpen }: RowProps) {
  const inApp = !!onOpen && !!knownKeys?.has(issue.key);
  const body = (
    <>
      {issue.issue_type && <TypeIcon type={issue.issue_type} />}
      <span className="task-rel-key">{issue.key}</span>
      <span className="task-rel-summary">{issue.summary}</span>
      {issue.status && (
        <span className="task-rel-pill" data-category={issue.status_category ?? "new"}>
          {issue.status}
        </span>
      )}
      {!inApp && (
        <>
          <ExternalLink size={12} aria-hidden="true" />
          <span className="task-sr">Opens in Jira</span>
        </>
      )}
    </>
  );
  if (inApp) {
    return (
      <button type="button" className="task-rel-row" onClick={() => onOpen(issue.key)}>
        {body}
      </button>
    );
  }
  return base ? (
    <a className="task-rel-row" href={`${base}/browse/${issue.key}`} target="_blank" rel="noreferrer">
      {body}
    </a>
  ) : (
    <span className="task-rel-row">{body}</span>
  );
}

/** First five, then a text button for the rest. */
function Capped<T>({ items, noun, render }: { items: T[]; noun: string; render: (item: T) => ReactNode }) {
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, FIRST);
  return (
    <>
      <ul className="task-rel-list">
        {shown.map((item, i) => (
          <li key={i}>{render(item)}</li>
        ))}
      </ul>
      {items.length > FIRST && !all && (
        <button type="button" className="task-rel-more" onClick={() => setAll(true)} aria-label={`Show all ${items.length} ${noun}`}>
          {`Show all ${items.length}`}
        </button>
      )}
    </>
  );
}

function File({ a }: { a: Attachment }) {
  const who = [a.author, a.created && formatStamp(a.created)].filter(Boolean).join(" · ");
  return (
    <a className="task-rel-row task-rel-file" href={a.url} target="_blank" rel="noreferrer">
      <Paperclip size={14} aria-hidden="true" />
      <span className="task-rel-summary">{a.filename}</span>
      <span className="task-rel-meta">{[fileSize(a.size_bytes), who].filter(Boolean).join(" · ")}</span>
      <span className="task-sr">Opens in a new tab</span>
    </a>
  );
}

function groupLinks(links: TicketDetail["links"]) {
  const groups = new Map<string, IssueRef[]>();
  for (const l of links) groups.set(l.relation, [...(groups.get(l.relation) ?? []), l.issue]);
  return [...groups];
}

export function TaskRelated({ detail, knownKeys, onOpen }: TaskRelatedProps) {
  if (!detail) return null;
  const { parent, subtasks, links, attachments } = detail;
  if (!parent && !subtasks.length && !links.length && !attachments.length) return null;
  const base = jiraBase(detail.url);
  const row = (issue: IssueRef) => <IssueRow issue={issue} base={base} knownKeys={knownKeys} onOpen={onOpen} />;
  const done = subtasks.filter((s) => s.status_category === "done").length;
  return (
    <section className="task-related" aria-labelledby="task-related-label">
      <h3 id="task-related-label" className="task-label">
        Related
      </h3>
      {parent && (
        <div className="task-rel-group">
          <h4 className="task-rel-head">Parent</h4>
          {row(parent)}
        </div>
      )}
      {subtasks.length > 0 && (
        <div className="task-rel-group">
          <h4 className="task-rel-head">{`Subtasks ${done} of ${subtasks.length} done`}</h4>
          <Capped items={subtasks} noun="subtasks" render={row} />
        </div>
      )}
      {groupLinks(links).map(([relation, issues]) => (
        <div key={relation} className="task-rel-group">
          <h4 className="task-rel-head">{sentence(relation)}</h4>
          <Capped items={issues} noun="linked issues" render={row} />
        </div>
      ))}
      {attachments.length > 0 && (
        <div className="task-rel-group">
          <h4 className="task-rel-head">Attachments</h4>
          <Capped items={attachments} noun="attachments" render={(a) => <File a={a} />} />
        </div>
      )}
    </section>
  );
}
