"use client";

import { useState } from "react";
import { draftStandup, postStandup, standupChannelSet } from "@/lib/daemonStandup";
import type { PostOutcome, StandupDraft } from "@/lib/daily_helpers_contract";

const QUESTIONS = [
  "What are you working on today?",
  "What is next/coming up?",
  "Are there any blockers we need to clear?",
];

/** Mirrors `StandupDraft::to_text` in the Rust contract. */
function draftText(d: StandupDraft): string {
  return [d.today, d.next, d.blockers]
    .map((bullets, i) => {
      const lines = bullets.length ? bullets : ["None"];
      return `${i + 1}. ${QUESTIONS[i]}\n${lines.map((b) => `• ${b}\n`).join("")}`;
    })
    .join("\n");
}

function failure(o: PostOutcome): string | null {
  switch (o.outcome) {
    case "posted":
      return null;
    case "no_channel":
      return "No Daily channel is set. Add one in Settings, or copy the text.";
    case "no_thread":
      return `No Daily thread found in ${o.channel} today. Copy the text and post it yourself.`;
    case "slack_refused":
      return `Slack refused the post (${o.error}). Copy the text and post it in the thread yourself.`;
  }
}

function Preview(p: {
  text: string;
  onText: (t: string) => void;
  canPost: boolean;
  channelSet: boolean;
  copied: boolean;
  permalink: string | null;
  onPost: () => void;
  onCopy: () => void;
  onDiscard: () => void;
}) {
  return (
    <div className="standup-preview">
      <textarea
        className="standup-draft"
        aria-label="Standup draft"
        rows={10}
        value={p.text}
        spellCheck={false}
        onChange={(e) => p.onText(e.target.value)}
      />
      {!p.channelSet && (
        <p role="status" className="standup-note">
          No Daily channel is set, so Post is off. Set one in Settings, or copy the text.
        </p>
      )}
      {p.channelSet && p.text.trim() === "" && (
        <p role="status" className="standup-note">
          The draft is empty, so Post is off. Write at least one line.
        </p>
      )}
      <div className="standup-actions">
        <button type="button" className="task-btn-primary" disabled={!p.canPost} onClick={p.onPost}>
          Post
        </button>
        <button type="button" className="task-btn-secondary" onClick={p.onCopy}>
          Copy
        </button>
        <button type="button" className="task-btn-secondary" onClick={p.onDiscard}>
          Discard draft
        </button>
        {p.copied && (
          <span role="status" className="standup-note">
            Copied to the clipboard
          </span>
        )}
      </div>
      {p.permalink && (
        <p role="status" className="standup-done">
          Posted to today&apos;s Daily thread. <a href={p.permalink}>Open thread</a>
        </p>
      )}
    </div>
  );
}

function useStandup() {
  const [busy, setBusy] = useState<"drafting" | "posting" | null>(null);
  const [copied, setCopied] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [channelSet, setChannelSet] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [permalink, setPermalink] = useState<string | null>(null);

  const start = async () => {
    setBusy("drafting");
    setError(null);
    setPermalink(null);
    setCopied(false);
    const [d, c] = await Promise.all([draftStandup(), standupChannelSet()]);
    setBusy(null);
    if (!d.ok) return setError(d.error);
    setChannelSet(c.ok ? c.data : true);
    setText(draftText(d.data));
  };

  const post = async () => {
    if (text === null) return;
    setBusy("posting");
    setError(null);
    const r = await postStandup(text);
    setBusy(null);
    if (!r.ok) return setError(r.error);
    setError(failure(r.data));
    if (r.data.outcome === "posted") setPermalink(r.data.permalink);
  };

  const copy = async () => {
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      setError("The browser blocked the clipboard. Select the draft and copy it by hand.");
    }
  };

  return { busy, copied, text, setText, setCopied, setPermalink, channelSet, error, permalink, start, post, copy };
}

export function StandupButton() {
  const { busy, copied, text, setText, setCopied, setPermalink, channelSet, error, permalink, start, post, copy } =
    useStandup();
  return (
    <div className="standup" aria-busy={busy !== null}>
      <button
        type="button"
        className="action-btn"
        disabled={busy !== null || text !== null}
        title={text !== null ? "Discard the open draft to start a new one" : undefined}
        onClick={() => void start()}
      >
        Standup
      </button>
      {busy && (
        <p role="status" className="standup-note">
          {busy === "drafting" ? "Drafting from yesterday and today…" : "Posting to the Daily thread…"}
        </p>
      )}
      {error && (
        <p role="alert" className="standup-error">
          {error}
        </p>
      )}
      {text !== null && (
        <Preview
          text={text}
          onText={(t) => {
            setText(t);
            setCopied(false);
            setPermalink(null);
          }}
          canPost={channelSet && busy === null && permalink === null && text.trim() !== ""}
          channelSet={channelSet}
          copied={copied}
          permalink={permalink}
          onPost={() => void post()}
          onCopy={() => void copy()}
          onDiscard={() => {
            setText(null);
            setCopied(false);
            setPermalink(null);
          }}
        />
      )}
    </div>
  );
}
