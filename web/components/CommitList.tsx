"use client";

import { useState, useTransition } from "react";
import { ChevronRight, ExternalLink, GitCommit } from "lucide-react";
import type { CommitEntry } from "@/lib/types";
import { fetchBlockCommits } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  blockId: number;
  isPersonal: boolean;
}

/**
 * Per-block commit drill-down. Mirrors `EventList`: collapsed by
 * default, lazy-fetches on first expand, caches on the component.
 *
 * For personal blocks we hide the chip entirely — the daemon would
 * return `[]` anyway and showing "0 commits" on a personal slot is
 * just noise. For work blocks we render the chip even before the
 * first fetch so the user can discover the affordance; the count
 * shows up after expansion.
 */
export function CommitList({ blockId, isPersonal }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [commits, setCommits] = useState<CommitEntry[] | null>(null);
  const [isPending, start] = useTransition();

  if (isPersonal) return null;

  const toggle = () => {
    if (!expanded && commits === null) {
      start(async () => {
        const r = await fetchBlockCommits(blockId);
        if (r.ok) {
          setCommits(r.data);
        } else {
          toast.error(`Load commits failed — ${r.error}`);
          return;
        }
      });
    }
    setExpanded((v) => !v);
  };

  const label =
    commits === null
      ? "commits"
      : commits.length === 1
        ? "1 commit"
        : `${commits.length} commits`;

  return (
    <div className="events-disclosure-wrap">
      <button
        type="button"
        className="events-disclosure"
        aria-expanded={expanded}
        aria-controls={`commits-list-${blockId}`}
        aria-busy={isPending || undefined}
        onClick={toggle}
      >
        <ChevronRight className={`disclosure-chev ${expanded ? "open" : ""}`} />
        <GitCommit width={11} height={11} strokeWidth={1.75} />
        {label}
      </button>

      {expanded && (
        <ul
          className="events-list commits-list"
          id={`commits-list-${blockId}`}
          role="list"
        >
          {commits === null ? (
            <li className="events-loading" aria-live="polite">
              loading…
            </li>
          ) : commits.length === 0 ? (
            <li className="events-loading">no commits in this window</li>
          ) : (
            commits.map((c) => <CommitRow key={c.sha} commit={c} />)
          )}
        </ul>
      )}
    </div>
  );
}

function CommitRow({ commit }: { commit: CommitEntry }) {
  const shaContent = (
    <span className="commit-sha" title={commit.sha}>
      {commit.short_sha}
    </span>
  );
  return (
    <li className="commit-row">
      {commit.github_url ? (
        <a
          href={commit.github_url}
          target="_blank"
          rel="noopener noreferrer"
          className="commit-sha-link"
          aria-label={`open commit ${commit.short_sha} on GitHub`}
        >
          {shaContent}
          <ExternalLink width={10} height={10} strokeWidth={1.75} />
        </a>
      ) : (
        shaContent
      )}
      <span className="commit-subject" title={commit.subject}>
        {commit.subject}
      </span>
      <span
        className="commit-stat"
        title={`${commit.files_changed} file${commit.files_changed === 1 ? "" : "s"} changed`}
        aria-label={`+${commit.insertions} insertions, -${commit.deletions} deletions`}
      >
        <span className="commit-ins">+{commit.insertions}</span>
        <span className="commit-del">-{commit.deletions}</span>
      </span>
    </li>
  );
}
